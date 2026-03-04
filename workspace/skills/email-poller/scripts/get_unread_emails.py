#!/usr/bin/env python3
"""
Get Unread Emails - Fetch unread emails from inbox.

Usage:
    python3 get_unread_emails.py [--page-size N] [--mark-as-read]

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

# Import mark_as_read function
import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mark_as_read import mark_as_read


def get_env_vars():
    """Get required environment variables."""
    return {
        'EWS_URL': os.environ.get('OUTLOOK_EWS_URL'),
        'USERNAME': os.environ.get('OUTLOOK_USERNAME'),
        'PASSWORD': os.environ.get('OUTLOOK_PASSWORD'),
        'DOMAIN': os.environ.get('OUTLOOK_DOMAIN'),
    }


def get_unread_emails(page_size=20):
    """获取未读邮件"""
    env = get_env_vars()

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
          <t:FieldURI FieldURI="message:IsRead"/>
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:Restriction>
        <t:IsEqualTo>
            <t:FieldURI FieldURI="message:IsRead"/>
            <t:FieldURIOrConstant>
                <t:Constant Value="false"/>
            </t:FieldURIOrConstant>
        </t:IsEqualTo>
      </m:Restriction>
      <m:IndexedPageItemView MaxEntriesReturned="{page_size}" Offset="0" BasePoint="Beginning"/>
      <m:ParentFolderIds>
        <t:DistinguishedFolderId Id="inbox"/>
      </m:ParentFolderIds>
    </m:FindItem>
  </soap:Body>
</soap:Envelope>'''

    session = requests.Session()
    session.auth = HttpNtlmAuth(f"{env['DOMAIN']}\\{env['USERNAME']}", env['PASSWORD'])
    session.headers.update({'Content-Type': 'text/xml; charset=utf-8'})

    response = session.post(env['EWS_URL'], data=soap_body, verify=False, timeout=30)
    response.raise_for_status()

    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(response.text.encode('utf-8'))
    items = root.xpath('//t:Items/t:Message', namespaces=namespaces)

    unread = []
    for item in items:
        item_id = item.find('t:ItemId', namespaces)
        if item_id is not None:
            unread.append({
                'item_id': item_id.get('Id'),
                'change_key': item_id.get('ChangeKey'),
                'subject': item.findtext('t:Subject', '', namespaces),
            })

    return unread


def main():
    parser = argparse.ArgumentParser(description='Get unread emails from Outlook via EWS')
    parser.add_argument('--page-size', type=int, default=20, help='Number of emails to fetch')
    parser.add_argument('--mark-as-read', action='store_true', help='Mark fetched emails as read')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    args = parser.parse_args()

    try:
        unread = get_unread_emails(page_size=args.page_size)

        if args.mark_as_read:
            for email in unread:
                try:
                    mark_as_read(email['item_id'], email['change_key'])
                except Exception as e:
                    print(f"Error marking {email['subject']} as read: {e}")

        if args.json:
            output = {
                'count': len(unread),
                'emails': unread,
                'marked_as_read': args.mark_as_read
            }
            print(json.dumps(output, indent=2, ensure_ascii=False))
        else:
            print(f"Unread emails: {len(unread)}")
            for email in unread:
                status = "[READ]" if args.mark_as_read else "[UNREAD]"
                print(f"  {status} {email['subject']}")
    except Exception as e:
        if args.json:
            print(json.dumps({'error': str(e)}))
        else:
            print(f"Error: {e}")


if __name__ == '__main__':
    main()
