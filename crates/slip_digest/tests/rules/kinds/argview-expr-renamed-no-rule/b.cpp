namespace n { constexpr int u = 0; }
template <int I, class T> int get(T);
template <class T> int f(T t) { return get<n::u>(t); }
