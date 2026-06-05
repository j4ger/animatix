#include <tree_sitter/parser.h>
#include <ctype.h>
#include <wctype.h>

enum TokenType {
    NUMBER,
    TIME_LITERAL,
};

void *tree_sitter_animatix_external_scanner_create() {
    return NULL;
}

void tree_sitter_animatix_external_scanner_destroy(void *payload) {
    (void)payload;
}

unsigned tree_sitter_animatix_external_scanner_serialize(void *payload, char *buffer) {
    (void)payload;
    (void)buffer;
    return 0;
}

void tree_sitter_animatix_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
    (void)payload;
    (void)buffer;
    (void)length;
}

bool tree_sitter_animatix_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid_symbols) {
    (void)payload;

    // Skip leading whitespace
    while (iswspace(lexer->lookahead)) {
        lexer->advance(lexer, true);
    }

    // Must start with a digit
    if (!isdigit(lexer->lookahead)) {
        return false;
    }

    // Scan the number part
    while (isdigit(lexer->lookahead)) {
        lexer->advance(lexer, false);
    }

    // Check for decimal point
    if (lexer->lookahead == '.') {
        lexer->advance(lexer, false);
        while (isdigit(lexer->lookahead)) {
            lexer->advance(lexer, false);
        }
    }

    // Now check what comes after the number
    // Case 1: 'ms' → time_literal
    if (lexer->lookahead == 'm') {
        lexer->advance(lexer, false);
        if (lexer->lookahead == 's') {
            lexer->advance(lexer, false);
            // Make sure it's not followed by alphanumeric or underscore
            if (iswalnum(lexer->lookahead) || lexer->lookahead == '_') {
                // Not a time literal, but we already consumed 'ms'
                // This is a lexer error, return false
                return false;
            }
            if (valid_symbols[TIME_LITERAL]) {
                lexer->result_symbol = TIME_LITERAL;
                return true;
            }
            // TIME_LITERAL not valid here, can't backtrack
            return false;
        }
        // 'm' not followed by 's' - can't be a number either since we consumed it
        return false;
    }

    // Case 2: 's' → time_literal
    if (lexer->lookahead == 's') {
        lexer->advance(lexer, false);
        // Make sure it's not followed by alphanumeric or underscore
        if (iswalnum(lexer->lookahead) || lexer->lookahead == '_') {
            // Not a time literal
            return false;
        }
        if (valid_symbols[TIME_LITERAL]) {
            lexer->result_symbol = TIME_LITERAL;
            return true;
        }
        // TIME_LITERAL not valid here
        return false;
    }

    // Case 3: just a number (not followed by s/ms)
    if (valid_symbols[NUMBER]) {
        lexer->result_symbol = NUMBER;
        return true;
    }

    return false;
}
