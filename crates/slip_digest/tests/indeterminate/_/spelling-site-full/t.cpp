#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/full`, a kind walked in full by profile mode (a lambda) inside a marked type.
int f(int);
template<class T> auto g(T t) -> decltype(f(t) + [] { return 0; }());
