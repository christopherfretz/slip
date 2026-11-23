#include "side.h"
// §4.4 "Spellings", rule 9: as `spelling-fold-operator-trailing`, a unary fold.
namespace N { struct X {}; }
int operator+(N::X, int);
#ifdef B
long operator+(N::X, long);
#endif
template<class... T> auto g(T... t) -> decltype((... + t));
