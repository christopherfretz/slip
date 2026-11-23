// std: c++17
// args: -Wno-dynamic-exception-spec
#include "side.h"
// §4.4 "Spellings", rule 2: the types of a dynamic exception specification of a function type that is not a declaration's own.
int f(int);
#ifdef B
long f(long);
#endif
template<class T> void g(T, void (*)() throw(decltype(f(T()))));
