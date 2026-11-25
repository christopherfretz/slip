#include "side.h"
// WP-156 fix round 1 (S4: mutation R30; `spec/digest.md` §2.4, S6): a
// parameter of a function type that no function declaration holds,
// named in an operand, is written by its depth and index (`Ref::param`),
// not as a local object: renaming it changes nothing.
int f(int);
long f(long);
#ifndef B
void (*fp)(int a, long b) noexcept(noexcept(f(b)));
using FT = void (int a, long b) noexcept(noexcept(f(a) + f(b)));
void g(void (*h)(int a, long b) noexcept(noexcept(f(b))));
#else
void (*fp)(int x, long y) noexcept(noexcept(f(y)));
using FT = void (int x, long y) noexcept(noexcept(f(x) + f(y)));
void g(void (*h)(int x, long y) noexcept(noexcept(f(y))));
#endif
