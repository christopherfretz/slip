#include "dep.h"

namespace acme {
struct Mine {};
template <class T> struct Box {};
} namespace legacy {
template <class T> struct traits<acme::Box<T>> {};
} namespace acme { template <> constexpr int width<Mine> = 4; } namespace legacy {
template <> int id<acme::Mine>(acme::Mine) { return 1; }
namespace lib { template <> struct hash<acme::Mine> {}; }
}
