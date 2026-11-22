#include "side.h"
// §5.7 ColdDeclData `dtor_delete` (demand, §11.5): a delete-expression of a class with a virtual destructor leaves every unit equal.
struct S { virtual ~S(); void operator delete(void*); };
#ifdef B
void y(S* p) { delete p; }
#endif
