#!/usr/bin/env python3
"""
Mark Email as Read - Mark an email as read via EWS.

Usage:
    python3 mark_as_read.py <item_id> <change_key>

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


def mark_as_read(item_id, change_key):
    """
    标记邮件为已读

    Args:
        item_id: 邮件 ItemId
        change_key: 邮件 ChangeKey

    Returns:
        bool: True if successful
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
    <m:UpdateItem MessageDisposition="SaveOnly" ConflictResolution="AlwaysOverwrite">
      <m:ItemChanges>
        <t:ItemChange>
          <t:ItemId Id="{item_id}" ChangeKey="{change_key}"/>
          <t:Updates>
            <t:SetItemField>
              <t:FieldURI FieldURI="message:IsRead"/>
              <t:Message>
                <t:IsRead>true</t:IsRead>
              </t:Message>
            </t:SetItemField>
          </t:Updates>
        </t:ItemChange>
      </m:ItemChanges>
    </m:UpdateItem>
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

    # Check response for errors
    namespaces = {
        'm': 'http://schemas.microsoft.com/exchange/services/2006/messages',
        't': 'http://schemas.microsoft.com/exchange/services/2006/types'
    }

    root = etree.fromstring(response.text.encode('utf-8'))
    error = root.xpath('//m:ResponseCode', namespaces=namespaces)

    if error and error[0].text != 'NoError':
        raise Exception(f"EWS Error: {error[0].text}")

    return True


def main():
    parser = argparse.ArgumentParser(description='Mark email as read via EWS')
    parser.add_argument('item_id', help='Email ItemId')
    parser.add_argument('change_key', help='Email ChangeKey')
    parser.add_argument('--json', action='store_true', help='Output in JSON format')
    args = parser.parse_args()

    try:
        result = mark_as_read(args.item_id, args.change_key)
        if args.json:
            print(json.dumps({'success': result, 'item_id': args.item_id}))
        else:
            print(f"Successfully marked {args.item_id} as read")
    except Exception as e:
        if args.json:
            print(json.dumps({'success': False, 'error': str(e)}))
        else:
            print(f"Error: {e}")


if __name__ == '__main__':
    main()
