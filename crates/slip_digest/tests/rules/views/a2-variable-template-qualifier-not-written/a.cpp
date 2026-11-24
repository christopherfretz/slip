namespace legacy { inline namespace in { template <class T> constexpr int v = sizeof(T); } }
int f() { return legacy::v<int>; }
namespace zzz { int z; }
