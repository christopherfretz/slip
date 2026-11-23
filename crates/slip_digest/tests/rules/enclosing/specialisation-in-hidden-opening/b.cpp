namespace d { template <class T> int g() { return 0; } }
namespace __attribute__((visibility("hidden"))) d { }
namespace d { template <> int g<int>() { return 1; } }
int u() { return d::g<int>(); }
