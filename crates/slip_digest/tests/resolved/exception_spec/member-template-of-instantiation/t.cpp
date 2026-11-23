#include "side.h"
// digest-kinds §11.2 rule 3 (WP-046 review F2): a member template of a
// class template's instantiation, and its specialisation. The member
// template's pattern in A<int> is made by instantiation from a pattern
// whose `noexcept` is dependent; the specialisation's pattern is that one
// (`SpecInfo`), whose specification Kiln holds `Deferred`.
template <class T> struct A { template <class U> void f() noexcept(sizeof(U) > 1); };
A<int> a;
#ifdef B
bool h() { return noexcept(a.f<long>()); }
#endif
