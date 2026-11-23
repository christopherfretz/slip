#include "side.h"
// §4.4 "Spellings", rule 5: as `spelling-conversion-name-decltype`, in a body walked in full.
int f(int);
#ifdef B
long f(long);
#endif
template<class T, class U> void g(T t, U u) { t.operator decltype(f(U()))(); }
