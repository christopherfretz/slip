#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): the two declaration orders of one class. Kiln makes a shadow of Base::f(), marked hidden, when the overrider follows the using-declaration, and none when it precedes it; the classes are the same ([namespace.udecl]/14), and with hidden shadows no units they digest alike.
struct Base { virtual void f(); void f(int); };
#ifdef B
struct D : Base { void f() override; using Base::f; };
#else
struct D : Base { using Base::f; void f() override; };
#endif
void h(D& d) { d.f(); d.f(1); }
