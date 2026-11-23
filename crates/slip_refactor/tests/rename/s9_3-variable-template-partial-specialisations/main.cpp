namespace n { template <class T> constexpr int v = 1; template <class T> constexpr int v<T*> = 2; template <> constexpr int v<int> = 3; }
template <class T> constexpr int n::v<T&> = 4;
template <class T> int g() { return n::v<T> + n::v<int>; }
int h() { return n::v<char> + n::v<int*> + g<long>(); }
