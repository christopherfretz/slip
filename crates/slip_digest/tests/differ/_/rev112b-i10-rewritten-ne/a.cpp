// rev-112b probe: a rewritten != through a consteval operator== in a default argument
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct Z {};
consteval bool operator==(Z, const char *s) { return len(s) == 1; }
void log(bool = Z{} != __builtin_FUNCTION());
void f() { log(); }
