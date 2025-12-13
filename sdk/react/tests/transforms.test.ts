import { describe, it, expect } from 'vitest';
import { transformSnakeToCamel, parseNumericStrings } from '../src/utils/transforms';

describe('transforms', () => {
  describe('transformSnakeToCamel', () => {
    it('should convert snake_case keys to camelCase', () => {
      const input = {
        first_name: 'John',
        last_name: 'Doe',
        email_address: 'john@example.com'
      };

      const result = transformSnakeToCamel(input);

      expect(result).toEqual({
        firstName: 'John',
        lastName: 'Doe',
        emailAddress: 'john@example.com'
      });
    });

    it('should handle nested objects', () => {
      const input = {
        user_info: {
          first_name: 'Jane',
          contact_details: {
            phone_number: '555-1234'
          }
        }
      };

      const result = transformSnakeToCamel(input);

      expect(result).toEqual({
        userInfo: {
          firstName: 'Jane',
          contactDetails: {
            phoneNumber: '555-1234'
          }
        }
      });
    });

    it('should handle arrays', () => {
      const input = [
        { first_name: 'John' },
        { first_name: 'Jane' }
      ];

      const result = transformSnakeToCamel(input);

      expect(result).toEqual([
        { firstName: 'John' },
        { firstName: 'Jane' }
      ]);
    });
  });

  describe('parseNumericStrings', () => {
    it('should parse numeric strings to numbers', () => {
      const input = {
        price: '123.45',
        quantity: '10',
        name: 'Product'
      };

      const result = parseNumericStrings(input);

      expect(result).toEqual({
        price: 123.45,
        quantity: 10,
        name: 'Product'
      });
    });

    it('should not parse empty strings', () => {
      const input = {
        value: '',
        name: 'Test'
      };

      const result = parseNumericStrings(input);

      expect(result).toEqual({
        value: '',
        name: 'Test'
      });
    });

    it('should handle nested structures', () => {
      const input = {
        data: {
          price: '99.99',
          items: [
            { count: '5' },
            { count: '10' }
          ]
        }
      };

      const result = parseNumericStrings(input);

      expect(result).toEqual({
        data: {
          price: 99.99,
          items: [
            { count: 5 },
            { count: 10 }
          ]
        }
      });
    });
  });
});
