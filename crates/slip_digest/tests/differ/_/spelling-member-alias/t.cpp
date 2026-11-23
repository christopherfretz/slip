#include "side.h"
// §4.4 "Spellings", rule 4: a member alias holds its found set, the member function using it the alias's Ref.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> struct S { using R = decltype(f(T())); void u(R r); };
