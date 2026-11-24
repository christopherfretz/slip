namespace legacy { inline namespace in { template <class T> constexpr int v = sizeof(T); } }
int f() { return legacy::in::v<int>; }
namespace yyy { int z; }
