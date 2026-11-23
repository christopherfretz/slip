// WP-115 item 5, C2 control (digest §9): a length read from a name string in a body from an included file is a value: the relocation changes it
void take(unsigned long);
consteval unsigned long len(const char *p) { unsigned long n = 0; while (p[n]) ++n; return n; }
namespace acme {
void f() {
#include "body.inc"
}
}
