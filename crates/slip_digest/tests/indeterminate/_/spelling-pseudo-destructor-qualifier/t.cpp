#include "side.h"
// §4.4 "Spellings", rule 11: a qualified pseudo-destructor (`p->Bq<..>::I::~I()`) in a body, its qualifier holding the found set of `f`: Kiln keeps that the name was qualified, not the qualifier, so the unit is indeterminate (`spelling/pseudo-destructor-qualifier`) rather than equal.
template<int N> struct Bq { using I = int; };
struct X {};
int f(int); int h(X);
#ifdef B
template<class T> void g(T t, int *p) { p->Bq<sizeof(sizeof(h(t)))>::I::~I(); }
#else
template<class T> void g(T t, int *p) { p->Bq<sizeof(sizeof(f(t)))>::I::~I(); }
#endif
