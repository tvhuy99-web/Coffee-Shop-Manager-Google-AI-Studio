package pl.mkrstudio.truefootball3.adapters

import android.content.Context
import android.graphics.Color
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ArrayAdapter
import android.widget.TextView
import pl.mkrstudio.truefootball3.R

class CommentaryRecordAdapter @Suppress("UNCHECKED_CAST") constructor(
    context: Context,
    resource: Int,
    objects: List<*>
) : ArrayAdapter<Any>(context, resource, objects as List<Any>) {

    override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
        val rowView = convertView ?: LayoutInflater.from(context)
            .inflate(R.layout.list_item_commentary_record, parent, false)
        val timeView = rowView.findViewById<TextView>(R.id.time)
        val commentaryView = rowView.findViewById<TextView>(R.id.commentary)
        timeView.setBackgroundColor(Color.TRANSPARENT)
        commentaryView.setBackgroundColor(Color.TRANSPARENT)
        val map = getItem(position) as? Map<*, *>
        val minute = map?.get("time")?.toString().orEmpty()
        val text = map?.get("commentary")?.toString().orEmpty()
        timeView.text = if (minute.isNotEmpty()) "$minute'" else ""
        commentaryView.text = text
        return rowView
    }
}
