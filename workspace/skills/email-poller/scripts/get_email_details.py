#!/usr/bin/env python3
"""
Get Email Details - Fetch full email content including body from Outlook via EWS.

Usage:
    python3 get_email_details.py <item_id> <change_key>

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


def get_email_details(item_id, change_key):
    """
    获取单封邮件的完整内容（包括 body）

    Args:
        item_id: 邮件 ItemId
        change_key: 邮件 ChangeKey

    Returns:
        dict with full email details
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
    <m:GetItem>
      <m:ItemShape>
        <t:BaseShape>AllProperties</t:BaseShape>
        <t:AdditionalProperties>
          <t:FieldURI FieldURI="message:Body"/>
          <t:FieldURI FieldURI="message:InternetMessageId"/>
          <t:FieldURI FieldURI="message:ToRecipients"/>
          <t:FieldURI FieldURI="message:CcRecipients"/>
        </t:AdditionalProperties>
      </m:ItemShape>
      <m:ItemIds>
        <t:ItemId Id="{item_id}" ChangeKey="{change_key}"/>
      </m:ItemIds>
    </m:GetItem>
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

    return parse_get_item_response(response.text)


def parse_get_item_response(xml_response):
    """解析 GetItem 响应"""
    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(xml_response.encode('utf-8'))
    message = root.xpath('//t:Message', namespaces=namespaces)

    if not message:
        return None

    msg = message[0]
    email = {
        'subject': '',
        'body_type': '',
        'body': '',
        'sender_name': '',
        'sender_email': '',
        'to_recipients': [],
        'cc_recipients': [],
        'received_time': '',
        'internet_message_id': ''
    }

    # 主题
    subject = msg.find('t:Subject', namespaces)
    if subject is not None:
        email['subject'] = subject.text or ''

    # Body（可能是 HTML 或纯文本）
    body = msg.find('t:Body', namespaces)
    if body is not None:
        email['body_type'] = body.get('BodyType', 'Text')
        email['body'] = body.text or ''

    # 发件人
    from_elem = msg.find('t:From', namespaces)
    if from_elem is not None:
        mailbox = from_elem.find('t:Mailbox', namespaces)
        if mailbox is not None:
            name = mailbox.find('t:Name', namespaces)
            email_addr = mailbox.find('t:EmailAddress', namespaces)
            if name is not None:
                email['sender_name'] = name.text or ''
            if email_addr is not None:
                email['sender_email'] = email_addr.text or ''

    # 收件人
    to_recipients = msg.find('t:ToRecipients', namespaces)
    if to_recipients is not None:
        for mailbox in to_recipients.xpath('t:Mailbox/t:EmailAddress', namespaces):
            email['to_recipients'].append(mailbox.text or '')

    # 抄送
    cc_recipients = msg.find('t:CcRecipients', namespaces)
    if cc_recipients is not None:
        for mailbox in cc_recipients.xpath('t:Mailbox/t:EmailAddress', namespaces):
            email['cc_recipients'].append(mailbox.text or '')

    # 接收时间
    received = msg.find('t:DateTimeReceived', namespaces)
    if received is not None:
        email['received_time'] = received.text or ''

    # Internet Message ID
    internet_msg_id = msg.find('t:InternetMessageId', namespaces)
    if internet_msg_id is not None:
        email['internet_message_id'] = internet_msg_id.text or ''

    return email


def main():
    parser = argparse.ArgumentParser(description='Get full email details from Outlook via EWS')
    parser.add_argument('item_id', help='Email ItemId')
    parser.add_argument('change_key', help='Email ChangeKey')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    parser.add_argument('--body-only', action='store_true', help='Output only the body content')
    args = parser.parse_args()

    email = get_email_details(args.item_id, args.change_key)

    if not email:
        print("Email not found")
        return

    if args.body_only:
        print(email['body'])
    elif args.json:
        print(json.dumps(email, indent=2, ensure_ascii=False))
    else:
        print(f"Subject: {email['subject']}")
        print(f"From: {email['sender_name']} <{email['sender_email']}>")
        print(f"To: {', '.join(email['to_recipients'])}")
        if email['cc_recipients']:
            print(f"Cc: {', '.join(email['cc_recipients'])}")
        print(f"Received: {email['received_time']}")
        print(f"Message ID: {email['internet_message_id']}")
        print(f"Body Type: {email['body_type']}")
        print("---")
        print(email['body'])


if __name__ == '__main__':
    main()
