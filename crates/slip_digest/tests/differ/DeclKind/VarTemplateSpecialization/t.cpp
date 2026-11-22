// std: c++17
#include "side.h"
// §5.3 DeclKind `VarTemplateSpecialization`: added.
template <class> int v = 0;
#ifdef B
template <> int v<int> = 1;
#endif
