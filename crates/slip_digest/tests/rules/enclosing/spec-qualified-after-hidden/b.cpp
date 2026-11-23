namespace d { template <class T> int g() { return 0; } }
namespace d { }
template <> int d::g<int>() { return 1; }
