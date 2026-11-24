package android.os
class Looper {companion object {fun getMainLooper()=Looper()}}
object TestClock {var now=0L;private data class Task(val at:Long,val r:Runnable);private val q=mutableListOf<Task>();fun add(r:Runnable,d:Long){q.add(Task(now+d,r))};fun remove(r:Runnable){q.removeAll{it.r===r}};fun advance(to:Long){var n=0;while(true){val t=q.filter{it.at<=to}.minByOrNull{it.at}?:break;check(++n<100000);q.remove(t);now=t.at;t.r.run()};now=to};fun reset(){q.clear();now=0}}
object SystemClock {fun elapsedRealtime()=TestClock.now}
class Handler(l:Looper){fun post(r:Runnable):Boolean{TestClock.add(r,0);return true};fun postDelayed(r:Runnable,d:Long):Boolean{TestClock.add(r,d);return true};fun removeCallbacks(r:Runnable)=TestClock.remove(r)}
