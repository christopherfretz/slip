// WP-115 item 5, C2 (digest §9): L::f()'s body ends in an included file, so its extent is not compared; a __func__ it may hold is a tie with the function around it of that name: the bytes are written, and renaming that function is equal
void take(const char *);
void g() {
struct L { void f() {
#include "tail.inc"
};
}
