#include "side.h"
// §4.4 "Spellings", rule 11: a qualified pseudo-destructor in a signature whose qualifier holds the found set of `f`, the object an `int *`: Kiln keeps that the name was qualified and not the qualifier (its type prints `decltype(p->~I() , t)`); profile mode raises the mark, and rule 1 makes the unit, held by the function template, indeterminate (`spelling/pseudo-destructor-qualifier`) rather than equal.
template<int N> struct Bq { using I = int; };
int f(int);
#ifdef B
long f(long);
#endif
template<class T> auto g(T t, int *p) -> decltype(p->Bq<sizeof(sizeof(f(t)))>::I::~I(), t) { return t; }
