#include "side.h"
// digest-kinds §11.2 rule 4, a specification not written: Kiln resolves
// the destructor's specification when something needs it. S::~S() writes
// none on either side; the resolved unit appears with the use.
struct S { ~S(); };
#ifdef B
bool h() { return noexcept(((S *)0)->~S()); }
#endif
