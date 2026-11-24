// rev-112b probe: an object's operator() with a consuming default
#include "srcloc.h"
consteval unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
void take(unsigned);
struct Log { void operator()(unsigned = len(__builtin_FUNCTION())) const; };
inline constexpr Log lg{};
void start() { lg(); }
