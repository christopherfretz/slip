namespace n { template <class T> constexpr bool v = true; }
template <class T> bool k() { return n::v<T>; }
