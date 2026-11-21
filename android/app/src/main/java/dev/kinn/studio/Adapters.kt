package dev.kinn.studio

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.ImageButton
import android.widget.TextView
import androidx.recyclerview.widget.RecyclerView

/**
 * List adapters for the Files and Problems sections.
 */

class ProjectsAdapter(
    private var items: List<String>,
    private var active: String?,
    private val onOpen: (String) -> Unit,
    private val onDelete: (String) -> Unit,
) : RecyclerView.Adapter<ProjectsAdapter.VH>() {

    class VH(v: View) : RecyclerView.ViewHolder(v) {
        val title: TextView = v.findViewById(R.id.project_title)
        val delete: ImageButton = v.findViewById(R.id.project_delete)
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH =
        VH(LayoutInflater.from(parent.context).inflate(R.layout.item_project, parent, false))

    override fun getItemCount(): Int = items.size

    override fun onBindViewHolder(holder: VH, position: Int) {
        val name = items[position]
        holder.title.text = if (name == active) "$name  (active)" else name
        holder.itemView.setOnClickListener { onOpen(name) }
        holder.delete.setOnClickListener { onDelete(name) }
    }

    fun update(newItems: List<String>, newActive: String?) {
        items = newItems
        active = newActive
        notifyDataSetChanged()
    }
}

class FilesAdapter(
    private var items: List<String>,
    private var active: String?,
    private val onOpen: (String) -> Unit,
    private val onDelete: (String) -> Unit,
    private val onRename: (String) -> Unit,
) : RecyclerView.Adapter<FilesAdapter.VH>() {

    class VH(v: View) : RecyclerView.ViewHolder(v) {
        val title: TextView = v.findViewById(R.id.file_title)
        val delete: ImageButton = v.findViewById(R.id.file_delete)
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH =
        VH(LayoutInflater.from(parent.context).inflate(R.layout.item_file, parent, false))

    override fun getItemCount(): Int = items.size

    override fun onBindViewHolder(holder: VH, position: Int) {
        val name = items[position]
        holder.title.text = if (name == active) "$name  (active)" else name
        holder.itemView.setOnClickListener { onOpen(name) }
        holder.itemView.setOnLongClickListener {
            onRename(name)
            true
        }
        holder.delete.setOnClickListener { onDelete(name) }
    }

    fun update(newItems: List<String>, newActive: String?) {
        items = newItems
        active = newActive
        notifyDataSetChanged()
    }
}

class ProblemsAdapter(
    private var items: List<Diagnostic>,
    private val onJump: (Diagnostic) -> Unit,
) : RecyclerView.Adapter<ProblemsAdapter.VH>() {

    class VH(v: View) : RecyclerView.ViewHolder(v) {
        val title: TextView = v.findViewById(R.id.problem_title)
        val location: TextView = v.findViewById(R.id.problem_location)
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): VH =
        VH(LayoutInflater.from(parent.context).inflate(R.layout.item_problem, parent, false))

    override fun getItemCount(): Int = items.size

    override fun onBindViewHolder(holder: VH, position: Int) {
        val d = items[position]
        holder.title.text = "${d.code}: ${d.message}"
        holder.title.setTextColor(
            when (d.severity) {
                "error" -> 0xFFF85149.toInt()
                "warning" -> 0xFFD29922.toInt()
                else -> 0xFF8B949E.toInt()
            }
        )
        holder.location.text = "${d.file.ifEmpty { "?" }}:${d.line}:${d.col}"
        holder.itemView.setOnClickListener { onJump(d) }
    }

    fun update(newItems: List<Diagnostic>) {
        items = newItems
        notifyDataSetChanged()
    }
}
