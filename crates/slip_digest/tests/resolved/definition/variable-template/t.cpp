#include "side.h"
// digest-kinds §11.9: the initialiser of a variable template
// specialisation is `definition`'s.
template <class T> T v = T(2);
#ifdef B
int h() { return v<int>; }
#else
int h() { return 0; }
#endif
