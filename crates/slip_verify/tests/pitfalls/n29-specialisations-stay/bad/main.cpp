#include "dep.h"

namespace acme {
struct Mine {};
template <class T> struct Box {};
} namespace legacy {
} namespace acme { template <class T> struct traits<Box<T>> {}; } namespace legacy {
template <> constexpr int width<acme::Mine> = 4;
template <> int id<acme::Mine>(acme::Mine) { return 1; }
namespace lib { template <> struct hash<acme::Mine> {}; }
}
