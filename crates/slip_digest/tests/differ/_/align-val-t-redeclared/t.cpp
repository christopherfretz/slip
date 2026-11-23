#include "side.h"
// WP-050: `<new>`'s shape: `std::align_val_t` and an aligned allocation
// function written in source after Kiln's implicit ones (declared without
// a location by the first declaration of a global `operator new`). Both
// are declared units of this file; `std` is a namespace (`tu`).
#ifdef B
namespace std { enum class align_val_t : decltype(sizeof 0) {}; }
void *operator new(decltype(sizeof 0), std::align_val_t);
#endif
int x;
