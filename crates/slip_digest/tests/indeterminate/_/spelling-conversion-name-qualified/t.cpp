#include "side.h"
// §4.4 "Spellings", rule 5: as `spelling-conversion-name-decltype`, for a qualified conversion name.
int f(int);
#ifdef B
long f(long);
#endif
template<class T, class U> auto g(T t, U u) -> decltype(T::operator decltype(f(U())));
