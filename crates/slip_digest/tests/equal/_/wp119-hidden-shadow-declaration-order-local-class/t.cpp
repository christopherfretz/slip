#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): wp119-hidden-shadow-declaration-order for a local class, whose members its function's unit lists (decl_class local_member): the hidden shadow is not listed, so the two orders digest alike. Kills the mutation "a local class lists its hidden shadows".
struct Base { virtual void f(); void f(int); };
int h() {
#ifdef B
  struct D : Base { void f() override {} using Base::f; };
#else
  struct D : Base { using Base::f; void f() override {} };
#endif
  D d; d.f(); d.f(1);
  return 0;
}
