#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 fix round 1 (S4: mutation R11): while an owner is numbered (a
// function with a local type), a stream walked and dropped (the immediate
// invocations of a default argument, `demand::immediates`) collects no
// operand of the owner's: the operand units are those of the declarations
// that hold the operands, once.
template <class T> struct Wr { T m; };
consteval int ci(int v) { return v; }
int g(int = ci(sizeof(Wr<decltype(make(x))>)));
int f() { auto l = [] { return 1; }; return g() + l(); }
int h() {
  auto l = [] { return 1; };
  return g(ci([] { struct In { Wr<decltype(make(x))> w; }; return int(sizeof(In)); }())) + l();
}
