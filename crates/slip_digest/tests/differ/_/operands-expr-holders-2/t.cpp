#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 fix round 1 (W4, W5, W9, the pseudo-destructor): the holders of
// a written type in an expression that the first delivery did not read.
template <class T> struct Wr { T m; };
template <class T> using V4 = T __attribute__((ext_vector_type(4)));
// W4: `__builtin_offsetof`.
unsigned long e_offs = __builtin_offsetof(Wr<decltype(make(x))>, m);
// W5: `__builtin_va_arg`.
int e_vaarg(int n, ...) {
  __builtin_va_list ap;
  __builtin_va_start(ap, n);
  int r = __builtin_va_arg(ap, decltype(make(x)));
  __builtin_va_end(ap);
  return r;
}
// A pseudo-destructor's destroyed type.
void e_pseudo(int *q) { q->~decltype(make(x))(); }
// W9: a requires-expression's type requirement and its parameter's type,
// outside a template.
bool e_req = requires { typename Wr<decltype(make(x))>; };
bool e_reqp = requires(decltype(make(x)) a) { a; };
// `__builtin_convertvector`'s written type.
V4<float> e_vf;
V4<int> e_cv() { return __builtin_convertvector(e_vf, V4<decltype(make(x))>); }
