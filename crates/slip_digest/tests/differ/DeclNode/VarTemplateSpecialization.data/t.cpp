// std: c++17
#include "side.h"
// §5.3 DeclNode `VarTemplateSpecialization.data`: the initialiser.
template <class> int v = 0;
#ifdef B
template <> int v<int> = 2;
#else
template <> int v<int> = 1;
#endif
