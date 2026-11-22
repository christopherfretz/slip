#include "side.h"
// §9.5 TemplateTables `member_like_friend`: a friend function template whose
// constraint names a parameter of the class template is mangled in the
// scope of the class specialisation.
template <class T> concept C = true;
template <class T> struct A {
#ifdef B
template <class U> friend void g(U) requires C<T> {}
#else
template <class U> friend void g(U) requires C<U> {}
#endif
};
A<int> a;
