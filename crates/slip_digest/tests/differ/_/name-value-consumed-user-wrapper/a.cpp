// WP-112 R2 (c) (digest §9): __builtin_FUNCTION() passed through the user immediate function id(...) in a default argument. It documents (c)'s false finding: id passes the name on, but Kiln evaluates the invocation once in the declaration's context (CWG 2631), so the digest cannot see that and writes the caller's name by value; the finding goes away when Kiln evaluates per use
#include "srcloc.h"
void take(const char *);
consteval const char *id(const char *p) { return p; }
void log(const char * = id(__builtin_FUNCTION()));
void f() { log(); }
