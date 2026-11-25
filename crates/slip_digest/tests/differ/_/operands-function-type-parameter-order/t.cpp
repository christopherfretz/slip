#include "side.h"
// WP-156 fix round 1 (S4: mutation R30; `spec/digest.md` §2.4, S6): a
// parameter of a function type that no function declaration holds,
// named in an operand, is written by its depth and index (`Ref::param`),
// not by the order the operand first names it (a local object's
// ordinal): which parameter an operand names is seen (`FT`, `fp`), and a
// parameter added before the function type around it changes neither
// (`gp`'s operand unit is equal: `b` is the first parameter of the inner
// function type on both sides). The parameters have one type, so only
// the reference tells the sides apart.
int f(int);
long f(long);
#ifndef B
using FT = void (int a, int b) noexcept(noexcept(f(a) + f(b)));
void (*fp)(int a, void (*q)(int b) noexcept(noexcept(f(a))));
void (*gp)(int a, void (*q)(int b) noexcept(noexcept(f(b))));
#else
using FT = void (int a, int b) noexcept(noexcept(f(b) + f(a)));
void (*fp)(int a, void (*q)(int b) noexcept(noexcept(f(b))));
void (*gp)(int a, int c, void (*q)(int b) noexcept(noexcept(f(b))));
#endif
