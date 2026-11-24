// WP-112 second fix round, F2 (digest §9 "Defaults at a use"): two functions whose default arguments call each other (with explicit arguments, so neither default reaches the other's: a default that reaches itself is ill-formed, "recursive evaluation of default argument"); the walk ends, and each use passes the caller's name on
#include "srcloc.h"
int b(int y, const void *p);
int a(int x = b(0, nullptr), const void *p = __builtin_source_location());
int b(int y = a(0, nullptr), const void *p = __builtin_source_location());
namespace acme {
int f() { return a() + b(); }
}
