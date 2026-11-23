namespace n { struct C { template <class T> static const int v = 1; template <class T> static const int v<T*> = 2; }; }
template <> const int n::C::v<int> = 3;
int h() { return n::C::v<long> + n::C::v<int*> + n::C::v<int>; }
