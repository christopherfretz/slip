namespace n { template <class T> int g() { return 0; } }
namespace __attribute__((visibility("hidden"))) n { template int g<int>(); }
