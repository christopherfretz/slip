#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
// WP-156 (S6): operands that name the parent's parameter, a local and
// `this`, written in the parent's numbering.
void f(lib::X p) { lib::X l; decltype(make(p)) a = 0; decltype(make(l)) b = 0; (void)a; (void)b; }
struct C { lib::X m; void g() { decltype(make(this->m)) c = 0; (void)c; } };
