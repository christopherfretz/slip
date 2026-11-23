#include "side.h"
// §4.4 "Spellings", rule 4: the alias holds its found set, its user the alias's Ref; f(long) added before the alias changes the alias only.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> using R = decltype(f(T()));
template<class T> void u(T t, R<T> r);
