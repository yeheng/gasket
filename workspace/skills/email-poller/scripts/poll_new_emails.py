#!/usr/bin/env python3
"""
Poll New Emails - Check for new emails since last check.

Usage:
    python3 poll_new_emails.py [--state-file PATH]

Environment Variables:
    OUTLOOK_EWS_URL   - EWS endpoint URL
    OUTLOOK_USERNAME  - Username
    OUTLOOK_PASSWORD  - Password
    OUTLOOK_DOMAIN    - Domain name
"""

import requests
from requests_ntlm import HttpNtlmAuth
from lxml import etree
import os
import argparse
import json
from datetime import datetime, timedelta


def get_env_vars():
    """Get required environment variables."""
    return {
        'EWS_URL': os.environ.get('OUTLOOK_EWS_URL'),
        'USERNAME': os.environ.get('OUTLOOK_USERNAME'),
        'PASSWORD': os.environ.get('OUTLOOK_PASSWORD'),
        'DOMAIN': os.environ.get('OUTLOOK_DOMAIN'),
    }


def poll_new_emails(state_file=None, hours_back=1):
    """
    轮询新邮件

    Args:
        state_file: 状态文件路径
        hours_back: 默认检查最近 N 小时

    Returns:
        list of new emails
    """
    env = get_env_vars()

    # 读取上次检查的时间戳
    if state_file is None:
        state_file = os.path.expanduser('~/.nanobot/email-poller-state.json')

    last_check = None
    if os.path.exists(state_file):
        with open(state_file, 'r') as f:
            state = json.load(f)
            last_check = state.get('last_check')

    # 如果没有上次检查时间，默认检查最近 hours_back 小时
    if not last_check:
        last_check = (datetime.utcnow() - timedelta(hours=hours_back)).isoformat() + 'Z'

    soap_body = f'''<?xml version="1.0" encoding="utf-8"?>
<soap:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
               xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages"
               xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"
               xmlns:soap="http://www.w3.org/2003/05/soap-envelope">
  <soap:Header>
    <t:RequestServerVersion Version="Exchange2010"/>
  </soap:Header>
  <soap:Body>
    <m:FindItem Traversal="Shallow">
      <m:ItemShape>
        <t:BaseShape>IdOnly</t:BaseShape>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="message:Subject"/>
          <t:FieldURI FieldURI="message:From"/>
          <t:FieldURI FieldURI="message:DateTimeReceived"/>
          <t:FieldURI FieldURI="message:IsRead"/>
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:Restriction>
        <t:IsGreaterThan>
            <t:FieldURI FieldURI="message:DateTimeReceived"/>
            <t:FieldURIOrConstant>
                <t:Constant Value="{last_check}"/>
            </t:FieldURIOrConstant>
        </t:IsGreaterThan>
      </m:Restriction>
      <m:IndexedPageItemView MaxEntriesReturned="100" Offset="0" BasePoint="Beginning"/>
      <m:ParentFolderIds>
        <t:DistinguishedFolderId Id="inbox"/>
      </m:ParentFolderIds>
    </m:FindItem>
  </soap:Body>
</soap:Envelope>'''

    session = requests.Session()
    session.auth = HttpNtlmAuth(f"{env['DOMAIN']}\\{env['USERNAME']}", env['PASSWORD'])
    session.headers.update({
        'Content-Type': 'text/xml; charset=utf-8',
    })

    response = session.post(env['EWS_URL'], data=soap_body, verify=False, timeout=30)

    if response.status_code != 200:
        raise Exception(f"EWS Error: {response.status_code}")

    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(response.text.encode('utf-8'))
    items = root.xpath('//t:Items/t:Message', namespaces=namespaces)

    new_emails = []
    for item in items:
        email = {
            'subject': item.findtext('t:Subject', '', namespaces),
            'received': item.findtext('t:DateTimeReceived', '', namespaces),
            'item_id': '',
            'change_key': '',
        }

        # Get ItemId
        item_id_elem = item.find('t:ItemId', namespaces)
        if item_id_elem is not None:
            email['item_id'] = item_id_elem.get('Id')
            email['change_key'] = item_id_elem.get('ChangeKey')

        from_elem = item.find('t:From', namespaces)
        if from_elem is not None:
            mailbox = from_elem.find('t:Mailbox', namespaces)
            if mailbox is not None:
                email['from'] = mailbox.findtext('t:EmailAddress', '', namespaces)

        new_emails.append(email)

    # 更新状态
    current_time = datetime.utcnow().isoformat() + 'Z'
    os.makedirs(os.path.dirname(state_file), exist_ok=True)
    with open(state_file, 'w') as f:
        json.dump({'last_check': current_time}, f)

    return new_emails


def main():
    parser = argparse.ArgumentParser(description='Poll new emails from Outlook via EWS')
    parser.add_argument('--state-file', type=str, help='State file path')
    parser.add_argument('--hours-back', type=int, default=1, help='Default hours to check if no state')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    args = parser.parse_args()

    try:
        new_emails = poll_new_emails(state_file=args.state_file, hours_back=args.hours_back)

        if args.json:
            print(json.dumps({
                'count': len(new_emails),
                'emails': new_emails
            }, indent=2, ensure_ascii=False))
        else:
            if new_emails:
                print(f"Found {len(new_emails)} new emails:")
                for email in reversed(new_emails):  # 最早的在前面
                    print(f"  [{email['received'][:16]}] {email['from']} - {email['subject']}")
            else:
                print("No new emails")
    except Exception as e:
        if args.json:
            print(json.dumps({'error': str(e)}))
        else:
            print(f"Error: {e}")


if __name__ == '__main__':
    main()
