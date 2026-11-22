#include "side.h"
// §5.4 LinkageLang `Cxx`: `extern "C++"` against no linkage specification.
#ifdef B
extern "C++" void f();
#else
void f();
#endif
