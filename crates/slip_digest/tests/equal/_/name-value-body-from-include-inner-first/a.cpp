// WP-115 item 5, C2 (digest §9): the inclusion holding the string is searched first: L::f() in the included file owns its __func__, not the function around the #include that has its plain name too; renaming that function is equal
void take(const char *);
void f() {
#include "body.inc"
}
