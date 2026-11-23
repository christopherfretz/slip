#include "side.h"
// digest-kinds §11.1 property 5: a destructor defined in source has its
// operator delete on both sides.
struct S { virtual ~S(); };
S::~S() {}
#ifdef B
void h() { S s; }
#endif
