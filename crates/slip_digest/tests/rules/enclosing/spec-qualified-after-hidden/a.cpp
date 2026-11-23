namespace d { template <class T> int g() { return 0; } }
namespace __attribute__((visibility("hidden"))) d { }
template <> int d::g<int>() { return 1; }
