namespace n { template <class T> int v = 0; }
namespace __attribute__((visibility("hidden"))) n { template <> int v<char> = 1; }
int u() { return n::v<char>; }
