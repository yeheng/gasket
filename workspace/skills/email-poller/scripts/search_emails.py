#!/usr/bin/env python3
"""
Search Emails - Search inbox emails with filters via EWS.

Usage:
    python3 search_emails.py [--subject KEYWORD] [--from ADDRESS] [--days-back N] [--page-size N]

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


def search_emails(subject_contains=None, from_address=None, days_back=7, page_size=50):
    """
    搜索收件箱邮件

    Args:
        subject_contains: 主题包含的关键字
        from_address: 发件人地址
        days_back: 搜索最近 N 天的邮件
        page_size: 返回数量

    Returns:
        list of matching emails
    """
    env = get_env_vars()

    # 计算日期范围
    start_date = (datetime.utcnow() - timedelta(days=days_back)).isoformat() + 'Z'
    end_date = datetime.utcnow().isoformat() + 'Z'

    # 构建搜索条件
    restrictions = []

    if subject_contains:
        restrictions.append(f'''
        <t:Contains ContainmentMode="Substring" ContainmentComparison="IgnoreCase">
            <t:FieldURI FieldURI="message:Subject"/>
            <t:Constant Value="{subject_contains}"/>
        </t:Contains>''')

    if from_address:
        restrictions.append(f'''
        <t:Contains ContainmentMode="Substring" ContainmentComparison="IgnoreCase">
            <t:FieldURI FieldURI="message:From"/>
            <t:Constant Value="{from_address}"/>
        </t:Contains>''')

    # 日期范围
    restrictions.append(f'''
    <t:IsGreaterThan>
        <t:FieldURI FieldURI="message:DateTimeReceived"/>
        <t:FieldURIOrConstant>
            <t:Constant Value="{start_date}"/>
        </t:FieldURIOrConstant>
    </t:IsGreaterThan>''')

    # 组合条件（AND）
    if len(restrictions) == 1:
        restriction_xml = restrictions[0]
    else:
        restriction_xml = '<t:And>' + ''.join(restrictions) + '</t:And>'

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
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:Restriction>
        {restriction_xml}
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
    session.headers.update({
        'Content-Type': 'text/xml; charset=utf-8',
        'Accept': 'application/xml',
    })

    response = session.post(env['EWS_URL'], data=soap_body, verify=False, timeout=30)
    response.raise_for_status()

    return parse_find_item_response(response.text)


def parse_find_item_response(xml_response):
    """解析响应"""
    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(xml_response.encode('utf-8'))
    items = root.xpath('//t:Items/t:Message', namespaces=namespaces)

    emails = []
    for item in items:
        email = {'subject': '', 'sender_name': '', 'sender_email': '', 'received_time': ''}

        subject = item.find('t:Subject', namespaces)
        if subject is not None:
            email['subject'] = subject.text or ''

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

        received = item.find('t:DateTimeReceived', namespaces)
        if received is not None:
            email['received_time'] = received.text or ''

        emails.append(email)

    return emails


def main():
    parser = argparse.ArgumentParser(description='Search emails in Outlook via EWS')
    parser.add_argument('--subject', type=str, help='Subject keyword to search')
    parser.add_argument('--from', dest='from_address', type=str, help='Sender email address')
    parser.add_argument('--days-back', type=int, default=7, help='Search last N days')
    parser.add_argument('--page-size', type=int, default=50, help='Number of results')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    args = parser.parse_args()

    emails = search_emails(
        subject_contains=args.subject,
        from_address=args.from_address,
        days_back=args.days_back,
        page_size=args.page_size
    )

    if args.json:
        print(json.dumps(emails, indent=2, ensure_ascii=False))
    else:
        print(f"Found {len(emails)} matching emails:")
        for email in emails:
            print(f"  {email['received_time'][:10]} | {email['sender_name']} | {email['subject']}")


if __name__ == '__main__':
    main()
