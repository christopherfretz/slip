#include "side.h"
// §9.1 TemplLinkKind `MemberOfSpecialization`: a member of a specialisation
// explicitly specialised has the link; a member of an explicit specialisation
// of the class has none.
template <class T> struct A { void f(); };
#ifdef B
template <> struct A<int> { void f(); };
void A<int>::f() {}
#else
template <> void A<int>::f() {}
#endif
