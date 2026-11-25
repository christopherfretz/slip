namespace n { constexpr int k = 0; }
template <class T, int I> struct It {};
template <class T> struct M { It<T, n::k>* p; };
