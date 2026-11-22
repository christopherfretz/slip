#include "side.h"
// §4.4, §7 _ `fold-init`: a fold with an init against one without.
#ifdef B
template<int... Ns> int v = (Ns + ... + 0);
#else
template<int... Ns> int v = (Ns + ...);
#endif
