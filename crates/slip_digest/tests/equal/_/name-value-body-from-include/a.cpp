// WP-115 item 5, C2 (digest §9): a function body that comes from an included file: the string's owner is found through the #include, so the relocation is equal
void take(const char *);
namespace legacy {
void f() {
#include "body.inc"
}
}
