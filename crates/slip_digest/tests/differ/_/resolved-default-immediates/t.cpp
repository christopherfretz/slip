#include "side.h"
// digest-kinds §11.1 property 7: the immediate invocation in f's default
// argument evaluates to a different value; f's own unit is the same.
#ifdef B
constexpr int k = 2;
#else
constexpr int k = 1;
#endif
consteval int id(int x) { return x + k; }
void f(int a = id(3));
void h() { f(); }
