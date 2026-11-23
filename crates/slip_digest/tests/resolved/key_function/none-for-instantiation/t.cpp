#include "side.h"
// digest-kinds §11.1 property 4: a template instantiation has no key
// function (Itanium C++ ABI §5.2.6); an explicit specialisation has one.
template <class T> struct S { virtual void f(); };
template <> struct S<char> { virtual void f(); };
#ifdef B
void h() { S<int> s; s.f(); }
#endif
