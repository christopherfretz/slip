#include "side.h"
// §5.7 ColdDeclData `dtor_delete_this` (demand, §11.5): a delete-expression of a class with a virtual destructor and a destroying or sized delete leaves every unit equal.

struct S { virtual ~S(); void operator delete(void*, decltype(sizeof 0)); };
#ifdef B
void y(S* p) { delete p; }
#endif
