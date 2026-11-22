// std: c++26
#include "side.h"
// §5.7 ColdDeclData `delete_message`: `= delete("reason")` against `= delete`.
#ifdef B
void f() = delete("no");
#else
void f() = delete;
#endif
