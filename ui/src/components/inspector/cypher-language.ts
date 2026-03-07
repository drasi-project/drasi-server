/**
 * Monarch tokenizer definitions for Cypher and GQL query languages.
 * Used with Monaco Editor for syntax highlighting in the Query Inspector.
 */
import type { languages } from "monaco-editor";

const cypherKeywords = [
  "MATCH",
  "OPTIONAL",
  "WHERE",
  "RETURN",
  "WITH",
  "UNWIND",
  "ORDER",
  "BY",
  "ASC",
  "ASCENDING",
  "DESC",
  "DESCENDING",
  "LIMIT",
  "SKIP",
  "CREATE",
  "MERGE",
  "DELETE",
  "DETACH",
  "REMOVE",
  "SET",
  "ON",
  "CASE",
  "WHEN",
  "THEN",
  "ELSE",
  "END",
  "CALL",
  "YIELD",
  "UNION",
  "ALL",
  "DISTINCT",
  "AS",
  "AND",
  "OR",
  "XOR",
  "NOT",
  "IN",
  "STARTS",
  "ENDS",
  "CONTAINS",
  "IS",
  "NULL",
  "TRUE",
  "FALSE",
  "EXISTS",
  "COUNT",
  "FOREACH",
  "LOAD",
  "CSV",
  "FROM",
  "FIELDTERMINATOR",
  "USING",
  "INDEX",
  "SCAN",
  "JOIN",
  "PERIODIC",
  "COMMIT",
  "CONSTRAINT",
  "ASSERT",
  "UNIQUE",
  "NODE",
  "RELATIONSHIP",
  "REL",
  "KEY",
];

const gqlExtraKeywords = [
  "SELECT",
  "INSERT",
  "UPDATE",
  "FOR",
  "LET",
  "FILTER",
  "GROUP",
  "HAVING",
  "MANDATORY",
  "GRAPH",
  "USE",
  "CATALOG",
  "SESSION",
  "SCHEMA",
  "ELEMENT",
  "PATH",
  "BINDING",
  "TABLE",
  "VALUE",
  "IF",
  "DO",
  "NEXT",
  "SHORTEST",
  "ANY",
  "SIMPLE",
  "ACYCLIC",
  "TRAIL",
  "WALK",
];

const builtinFunctions = [
  "count",
  "sum",
  "avg",
  "min",
  "max",
  "collect",
  "size",
  "length",
  "type",
  "id",
  "elementId",
  "labels",
  "nodes",
  "relationships",
  "properties",
  "keys",
  "startNode",
  "endNode",
  "head",
  "last",
  "tail",
  "range",
  "reverse",
  "reduce",
  "abs",
  "ceil",
  "floor",
  "round",
  "sign",
  "rand",
  "log",
  "log10",
  "exp",
  "sqrt",
  "toInteger",
  "toFloat",
  "toString",
  "toBoolean",
  "trim",
  "ltrim",
  "rtrim",
  "replace",
  "split",
  "substring",
  "toLower",
  "toUpper",
  "left",
  "right",
  "coalesce",
  "timestamp",
  "date",
  "datetime",
  "time",
  "duration",
  "point",
  "distance",
  "exists",
];

function createTokensProvider(
  keywords: string[]
): languages.IMonarchLanguage {
  return {
    defaultToken: "",
    ignoreCase: true,
    keywords,
    builtinFunctions,
    operators: [
      "=",
      "<>",
      "!=",
      "<",
      ">",
      "<=",
      ">=",
      "=~",
      "+",
      "-",
      "*",
      "/",
      "%",
      "^",
      "..",
    ],

    tokenizer: {
      root: [
        // Whitespace & comments
        [/\/\/.*$/, "comment"],
        [/\/\*/, "comment", "@blockComment"],

        // Strings
        [/"/, "string", "@doubleString"],
        [/'/, "string", "@singleString"],

        // Numbers
        [/\d+\.\d*([eE][-+]?\d+)?/, "number.float"],
        [/\d+([eE][-+]?\d+)?/, "number"],

        // Node labels and relationship types  :Label
        [/:([A-Za-z_]\w*)/, "type.identifier"],

        // Parameters  $param
        [/\$[A-Za-z_]\w*/, "variable"],

        // Function calls  name(
        [
          /[a-zA-Z_]\w*(?=\s*\()/,
          {
            cases: {
              "@keywords": "keyword",
              "@builtinFunctions": "support.function",
              "@default": "identifier",
            },
          },
        ],

        // Keywords & identifiers
        [
          /[a-zA-Z_]\w*/,
          {
            cases: {
              "@keywords": "keyword",
              "@default": "identifier",
            },
          },
        ],

        // Operators & brackets
        [/[<>]=?|<>|!=|=~|=/, "operator"],
        [/[+\-*/%^]/, "operator"],
        [/\.\./, "operator"],
        [/[()[\]{}]/, "delimiter.bracket"],
        [/[,;.]/, "delimiter"],
      ],

      blockComment: [
        [/[^/*]+/, "comment"],
        [/\*\//, "comment", "@pop"],
        [/[/*]/, "comment"],
      ],

      doubleString: [
        [/[^"\\]+/, "string"],
        [/\\./, "string.escape"],
        [/"/, "string", "@pop"],
      ],

      singleString: [
        [/[^'\\]+/, "string"],
        [/\\./, "string.escape"],
        [/'/, "string", "@pop"],
      ],
    },
  } as languages.IMonarchLanguage;
}

export const cypherLanguageId = "cypher";
export const gqlLanguageId = "gql";

export const cypherTokensProvider = createTokensProvider(cypherKeywords);
export const gqlTokensProvider = createTokensProvider([
  ...cypherKeywords,
  ...gqlExtraKeywords,
]);

export const cypherLanguageConfig: languages.LanguageConfiguration = {
  comments: { lineComment: "//", blockComment: ["/*", "*/"] },
  brackets: [
    ["(", ")"],
    ["[", "]"],
    ["{", "}"],
  ],
  autoClosingPairs: [
    { open: "(", close: ")" },
    { open: "[", close: "]" },
    { open: "{", close: "}" },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
  surroundingPairs: [
    { open: "(", close: ")" },
    { open: "[", close: "]" },
    { open: "{", close: "}" },
    { open: '"', close: '"' },
    { open: "'", close: "'" },
  ],
};
