#include "side.h"
// §4.4 "Spellings", rule 9: Kiln records no definition-context operator set for a fold: `spelling/fold-operator`.
namespace N { struct X {}; }
int operator+(N::X, int);
#ifdef B
long operator+(N::X, long);
#endif
template<class... T> auto g(T... t) -> decltype((t + ... + 0L));
