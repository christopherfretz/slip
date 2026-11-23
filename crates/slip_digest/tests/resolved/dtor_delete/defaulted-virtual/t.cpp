#include "side.h"
// digest-kinds §11.1 property 5: Kiln looks up a virtual destructor's
// operator delete when it defines the destructor, here on its first use.
struct S { virtual ~S() = default; };
#ifdef B
void h() { S s; }
#endif
