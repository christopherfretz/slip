#include "side.h"
// WP-155: a variable with `used` is emitted unreferenced, however it is
// initialised.
__attribute__((used)) static constexpr int k = 1;
