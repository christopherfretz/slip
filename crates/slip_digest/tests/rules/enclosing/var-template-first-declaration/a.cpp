namespace __attribute__((visibility("hidden"))) n { template <class T> extern int v; }
namespace n { template <class T> int v = 1; }
int u() { return n::v<int>; }
