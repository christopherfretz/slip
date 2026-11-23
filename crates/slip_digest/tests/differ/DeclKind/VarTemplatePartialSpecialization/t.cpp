// std: c++17
#include "side.h"
// §5.3 DeclKind `VarTemplatePartialSpecialization`: added.
template <class> int v = 0;
#ifdef B
template <class T> int v<T *> = 1;
#endif
