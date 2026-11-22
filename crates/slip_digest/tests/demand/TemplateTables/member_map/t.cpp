#include "side.h"
// §9.5 TemplateTables `member_map` (§11.8): a use of a member of an
// instantiation maps it to its pattern member; no unit both sides have
// changes.
template <class T> struct X { static int f() { return 1; } };
#ifdef B
int h() { return X<int>::f(); }
#else

#endif
