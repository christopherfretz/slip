#include "side.h"
// §9.1 TemplLinkKind `Specialization`: an explicit specialisation of a member
// function template against a plain member function.
#ifdef B
struct A { template <class T> void f(T); };
template <> void A::f<int>(int) {}
#else
struct A { void f(int); };
void A::f(int) {}
#endif
