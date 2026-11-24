#include "d.h"
#ifdef POST
namespace fresh { struct Mine {}; }
template <> struct old::box<fresh::Mine> { int v; };
#else
namespace old { struct Mine {}; }
template <> struct old::box<old::Mine> { int v; };
#endif
