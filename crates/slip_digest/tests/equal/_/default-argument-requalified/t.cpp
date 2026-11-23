#include "side.h"
// digest §1 rule 2, digest-kinds §11.4: a requalified name in a default
// argument of a class template's member leaves every unit equal, the
// instantiated `default_argument` unit included.
namespace ns {
int x;
template <class T> struct A {
#ifdef B
  void f(int a = ::ns::x);
#else
  void f(int a = x);
#endif
};
}
ns::A<int> a;
void h() { a.f(); }
