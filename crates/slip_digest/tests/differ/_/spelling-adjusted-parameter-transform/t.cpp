#include "side.h"
// As `spelling-adjusted-parameter`, a unary transform (s07).
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, __underlying_type(decltype(f(T()))) [2]);
