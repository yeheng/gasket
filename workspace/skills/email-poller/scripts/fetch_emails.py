#!/usr/bin/env python3
"""
Email Poller Helper - Fetch emails from Outlook via EWS with NTLM authentication.

Usage:
    python3 fetch_emails.py [--page-size N] [--offset N]

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


def get_env_vars():
    """Get required environment variables."""
    return {
        'EWS_URL': os.environ.get('OUTLOOK_EWS_URL'),
        'USERNAME': os.environ.get('OUTLOOK_USERNAME'),
        'PASSWORD': os.environ.get('OUTLOOK_PASSWORD'),
        'DOMAIN': os.environ.get('OUTLOOK_DOMAIN'),
    }


def fetch_emails(page_size=50, offset=0):
    """
    分页获取收件箱邮件

    Args:
        page_size: 每页数量（默认 50，最大 1000）
        offset: 偏移量（用于分页）

    Returns:
        list of dict with subject, sender, received_time
    """
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
          <t:FieldURI FieldURI="message:Sender"/>
          <t:FieldURI FieldURI="message:DateTimeReceived"/>
          <t:FieldURI FieldURI="message:IsRead"/>
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:IndexedPageItemView MaxEntriesReturned="{page_size}" Offset="{offset}" BasePoint="Beginning"/>
      <m:ParentFolderIds>
        <t:DistinguishedFolderId Id="inbox">
          <t:Mailbox>
            <t:EmailAddress>{env['USERNAME']}@{env['DOMAIN']}.com</t:EmailAddress>
          </t:Mailbox>
        </t:DistinguishedFolderId>
      </m:ParentFolderIds>
      <m:SortOrder>
        <t:FieldOrder Order="Descending">
          <t:FieldURI FieldURI="message:DateTimeReceived"/>
        </t:FieldOrder>
      </m:SortOrder>
    </m:FindItem>
  </soap:Body>
</soap:Envelope>'''

    session = requests.Session()
    session.auth = HttpNtlmAuth(f"{env['DOMAIN']}\\{env['USERNAME']}", env['PASSWORD'])
    session.headers.update({
        'Content-Type': 'text/xml; charset=utf-8',
        'Accept': 'application/xml',
    })

    response = session.post(env['EWS_URL'], data=soap_body, verify=False, timeout=30)
    response.raise_for_status()

    return parse_find_item_response(response.text)


def parse_find_item_response(xml_response):
    """解析 FindItem 响应"""
    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(xml_response.encode('utf-8'))
    items = root.xpath('//t:Items/t:Message', namespaces=namespaces)

    emails = []
    for item in items:
        email = {
            'subject': '',
            'sender_name': '',
            'sender_email': '',
            'received_time': '',
            'is_read': False,
            'item_id': '',
            'change_key': ''
        }

        # 提取 ItemId
        item_id_elem = item.find('t:ItemId', namespaces)
        if item_id_elem is not None:
            email['item_id'] = item_id_elem.get('Id', '')
            email['change_key'] = item_id_elem.get('ChangeKey', '')

        # 提取主题
        subject = item.find('t:Subject', namespaces)
        if subject is not None:
            email['subject'] = subject.text or ''

        # 提取发件人
        from_elem = item.find('t:From', namespaces)
        if from_elem is not None:
            mailbox = from_elem.find('t:Mailbox', namespaces)
            if mailbox is not None:
                name = mailbox.find('t:Name', namespaces)
                email_addr = mailbox.find('t:EmailAddress', namespaces)
                if name is not None:
                    email['sender_name'] = name.text or ''
                if email_addr is not None:
                    email['sender_email'] = email_addr.text or ''

        # 提取接收时间
        received = item.find('t:DateTimeReceived', namespaces)
        if received is not None:
            email['received_time'] = received.text or ''

        # 提取已读状态
        is_read = item.find('t:IsRead', namespaces)
        if is_read is not None:
            email['is_read'] = is_read.text == 'true'

        emails.append(email)

    return emails


def main():
    parser = argparse.ArgumentParser(description='Fetch emails from Outlook via EWS')
    parser.add_argument('--page-size', type=int, default=50, help='Number of emails to fetch')
    parser.add_argument('--offset', type=int, default=0, help='Offset for pagination')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    args = parser.parse_args()

    emails = fetch_emails(page_size=args.page_size, offset=args.offset)

    if args.json:
        print(json.dumps(emails, indent=2, ensure_ascii=False))
    else:
        for email in emails:
            status = '✓' if email['is_read'] else '○'
            print(f"[{status}] {email['received_time']}")
            print(f"    From: {email['sender_name']} <{email['sender_email']}>")
            print(f"    Subject: {email['subject']}")
            print(f"    ItemId: {email['item_id']}")
            print()


if __name__ == '__main__':
    main()
