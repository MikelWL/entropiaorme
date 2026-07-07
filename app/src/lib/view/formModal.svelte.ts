export interface FormModalOptions<TForm, TEntity> {
	/** Fresh form state for a create flow; called anew on every `openNew`. */
	blank: () => TForm;
	/** Persist the form; `editing` is null on create, the entity on edit. */
	save: (form: TForm, editing: TEntity | null) => Promise<void>;
}

export interface FormModal<TForm, TEntity> {
	readonly open: boolean;
	readonly editing: TEntity | null;
	form: TForm;
	readonly saving: boolean;
	readonly error: string | null;
	openNew(): void;
	openEdit(entity: TEntity, toForm: (entity: TEntity) => TForm): void;
	close(): void;
	submit(): Promise<void>;
}

/**
 * Modal-with-local-form lifecycle view model: create and edit flows share one
 * form, `submit` guards double submission, captures a thrown error's message,
 * closes on success and stays open (error shown) on failure.
 */
export function createFormModal<TForm, TEntity>(
	options: FormModalOptions<TForm, TEntity>,
): FormModal<TForm, TEntity> {
	let open = $state(false);
	let editing = $state<TEntity | null>(null);
	let form = $state<TForm>(options.blank());
	let saving = $state(false);
	let error = $state<string | null>(null);

	return {
		get open() {
			return open;
		},
		get editing() {
			return editing;
		},
		get form() {
			return form;
		},
		set form(value: TForm) {
			form = value;
		},
		get saving() {
			return saving;
		},
		get error() {
			return error;
		},
		openNew() {
			editing = null;
			form = options.blank();
			error = null;
			open = true;
		},
		openEdit(entity: TEntity, toForm: (entity: TEntity) => TForm) {
			editing = entity;
			form = toForm(entity);
			error = null;
			open = true;
		},
		close() {
			open = false;
			editing = null;
		},
		async submit() {
			if (saving) return;
			saving = true;
			error = null;
			try {
				await options.save(form, editing);
				open = false;
				editing = null;
			} catch (e) {
				error = e instanceof Error ? e.message : String(e);
			} finally {
				saving = false;
			}
		},
	};
}
