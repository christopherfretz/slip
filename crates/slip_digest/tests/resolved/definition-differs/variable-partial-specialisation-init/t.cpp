#include "side.h"
// digest-kinds §11.9 (WP-045 review, N17): v<int*> is instantiated from
// the partial specialisation `SpecInfo` `from` names, which has the
// initialiser; the primary has none.
template <class T> T v;
#ifdef B
template <class T> T *v<T *> = nullptr;
#else
template <class T> T *v<T *> = 0;
#endif
int *h() { return v<int *>; }
