#include "side.h"
// digest-kinds §11.9 (WP-045 review, N17): the pattern of v<int*> is the
// partial specialisation (`SpecInfo` `from`), which has an initialiser:
// `definition` waits for it, and no unit moves with the use.
template <class T> T v;
template <class T> T *v<T *> = nullptr;
using U = decltype(v<int *>);
#ifdef B
int *u() { return v<int *>; }
#endif
