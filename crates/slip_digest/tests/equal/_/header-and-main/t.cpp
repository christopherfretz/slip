#include "side.h"
// WP-024 Tests: an entity declared in a header and defined in the main file
// is one unit, in the header's group: the same as when it is also defined
// in the header (side B).
#include "h.h"
#ifndef B
void f(int) {}
#endif
