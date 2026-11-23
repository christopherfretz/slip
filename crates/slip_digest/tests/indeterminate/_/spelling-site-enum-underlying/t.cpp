#include "side.h"
// §4.4 "Spellings", rule 5: `spelling/decltype`, the fixed underlying type of an enumeration is held canonical.
int f(int);
template<class T> struct E { enum class K : decltype(f(T())) {}; };
