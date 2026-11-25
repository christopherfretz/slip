namespace n { constexpr int u = 0; }
template <class T, int I> struct It {};
template <class T> struct M { It<T, n::u>* p; };
